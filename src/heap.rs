use std::collections::HashMap;
use crate::object::{ObjClosure, ObjFunction, ObjNativeFunction, ObjString, ObjUpvalue};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ObjRef(usize);

// All heap-allocated object types
pub enum HeapObj {
    String(ObjString),
    Function(ObjFunction),
    NativeFunction(ObjNativeFunction),
    Closure(ObjClosure),
    Upvalue(ObjUpvalue)
}

impl HeapObj {
    pub fn type_name(&self) -> &'static str {
        match self {
            HeapObj::String(_) => "String",
            HeapObj::Function(_) => "Function",
            HeapObj::NativeFunction(_) => "NativeFunction",
            HeapObj::Closure(_) => "Closure",
            HeapObj::Upvalue(_) => "Upvalue",
        }
    }

    pub fn as_string(&self) -> &ObjString {
        match self {
            HeapObj::String(v) => v,
            _ => panic!("Expected String")
        }
    }
    pub fn as_string_mut(&mut self) -> &mut ObjString {
        match self {
            HeapObj::String(v) => v,
            _ => panic!("Expected String")
        }
    }
    pub fn as_function(&self) -> &ObjFunction {
        match self {
            HeapObj::Function(v) => v,
            _ => panic!("Expected Function")
        }
    }
    pub fn as_function_mut(&mut self) -> &mut ObjFunction {
        match self {
            HeapObj::Function(v) => v,
            _ => panic!("Expected Function")
        }
    }
    pub fn as_native_function(&self) -> &ObjNativeFunction {
        match self {
            HeapObj::NativeFunction(v) => v,
            _ => panic!("Expected NativeFunction")
        }
    }
    pub fn as_native_function_mut(&mut self) -> &mut ObjNativeFunction {
        match self {
            HeapObj::NativeFunction(v) => v,
            _ => panic!("Expected NativeFunction")
        }
    }
    pub fn as_closure(&self) -> &ObjClosure {
        match self {
            HeapObj::Closure(v) => v,
            _ => panic!("Expected Closure")
        }
    }
    pub fn as_closure_mut(&mut self) -> &mut ObjClosure {
        match self {
            HeapObj::Closure(v) => v,
            _ => panic!("Expected Closure")
        }
    }
    pub fn as_upvalue(&self) -> &ObjUpvalue {
        match self {
            HeapObj::Upvalue(v) => v,
            _ => panic!("Expected Upvalue")
        }
    }
    pub fn as_upvalue_mut(&mut self) -> &mut ObjUpvalue {
        match self {
            HeapObj::Upvalue(v) => v,
            _ => panic!("Expected Upvalue")
        }
    }
}

/*
* Used to manage Lox runtime objects the VM operates on
 */
pub struct Heap {
    objects: Vec<Option<HeapObj>>,
    free_slots: Vec<usize>, // reusable indices

    // String interning
    strings: HashMap<String, ObjRef>,

    // GC mark phase
    gray_stack: Vec<ObjRef>,

    // GC marked vec
    marked_objs: Vec<bool>,

    /*
    * In order to understand when to run the GC, is necessary to reason about:
    *   - throughput
    *   - latency
    *
    * Every managed language, wastes some time performing garbage collection with respect
    * to performing manual memory management.
    * The wasted time is due to the gc figuring out what to free (mark phase).
    * The throughput and the latency are two metrics which help to understand in which way
    * this time affects the performances of the program.
    * Throughput represents the total time spent running the program vs executing garbage collection.
    * If the program runs for 1 second and of this 1 second 0.1 is spent running the gc, the actual
    * program runs for 0.9 => 90% throughput.
    * The latency instead is the max continuous amount of time that the program is blocked doing gc
    * work.
    * If a program stops for 1 second to perform gc, it has a latency of 1 sec, if a program stops
    * 5 times for 0.2 seconds to perform gc, it has a latency of 0.2 seconds.
    * The throughput is unchanged.
    * To control these two metrics and find the right trade-off, is necessary to understand the frequency
    * at which to run the gc.
    * In particular the gc implemented in this case is a "stop the world" gc meaning, while it is running
    * the program does not proceed, is stuck.
    * If the gc is run too infrequently, then a big amount of unreachable objects accumulates, which will
    * make the gc to "stop the world" for a long time when it will run => big latency.
    * If however the gc is run too frequently, it could be that it spend lots of time visiting reachable
    * objects, because the program does not have time to make live objects unreachable.
    * This will cause a lot of time waste and throughput will suffer.
    * Is all a matter of finding the right balance.
    * The technique used here, is to adjust the frequency of gc run based on the size of the heap.
    * The number of bytes of managed memory is tracked, when it goes above a certain threshold the gc
    * is called.
    * Once the gc has run, the size of managed memory is recomputed and a new threshold above that value
    * is set.
    * The idea is that as the amount of LIVE (reachable) memory goes up the gc is called less frequently to avoid
    * losing throughput, while as the amount of reachable memory goes down the gc is called more frequently
    * to avoid big latencies.
    *
     */
    bytes_allocated: usize,
    next_gc: usize
}

impl Heap {
    pub fn new() -> Self {
        Heap {
            objects: Vec::new(),
            free_slots: Vec::new(),
            strings: HashMap::new(),
            gray_stack: Vec::new(),
            marked_objs: Vec::new(),
            bytes_allocated: 0,
            next_gc: 1024 * 1024
        }
    }
    
    pub fn alloc(&mut self, obj: HeapObj) -> ObjRef {
        // increment bytes allocated
        let size = size_of_val(&obj);
        self.bytes_allocated = self.bytes_allocated + size;

        let index = if let Some(free) = self.free_slots.pop() {
            self.objects[free] = Some(obj);
            self.marked_objs[free] = false;
            free
        } else {
            self.objects.push(Some(obj));
            self.marked_objs.push(false);
            self.objects.len() - 1
        };
        ObjRef(index)
    }
    
    pub fn get_mut(&mut self, handle: ObjRef) -> &mut HeapObj {
        self.objects[handle.0].as_mut().expect("dangling ObjRef")
    }
    
    pub fn get(&self, handle: ObjRef) -> &HeapObj {
        self.objects[handle.0].as_ref().expect("dangling ObjRef")
    }

    pub fn get_bytes_allocated(&self) -> usize {
        self.bytes_allocated
    }

    pub fn get_next_gc(&self) -> usize {
        self.next_gc
    }

    pub fn set_bytes_allocated(&mut self, bytes_allocated: usize) {
        self.bytes_allocated = bytes_allocated;
    }

    pub fn set_next_gc(&mut self, next_gc: usize) {
        self.next_gc = next_gc;
    }

    /*
    * The algorithm to mark all the reachable objects can be summarized with a tricolor abstraction.
    * Each object, can have 3 colors:
    *   - white: At the beginning of the gc, every object is white, it means no objects has been reached yet.
    *   - gray: Once the gc iterates over an object, it becomes gray. At this point the gc iterated over the
    *           object, but not through the objects referenced by it, which are still white.
    *   - black: Once the gc has processed all the objects that where referenced by a gray object, that
    *            gray object becomes black.
    *
    * At the end of the gc phase, all the objects left white, can be freed as they are unreachable.
    * Practically speaking the three colors translate as follows:
    *
    *   - An object that is not in self.gray_stack and whose value in self.marked_objs = false is white
    *   - An object in self.gray_stack and whose value in self.marked_objs = true is gray
    *   - An object whose value in self.marked_objs = true but is no longer in self.gray_stack is black
    *
     */
    pub fn mark_object(&mut self, obj_ref: ObjRef) {
        let index = obj_ref.0;
        /*
        *
        * The object is already gray or black
        * this could happen in the case of cycles
        * object A references B which references A
        * without this condition, a gray object could still
        * be added again in the gray stack or a black object
        * could be turned back to gray, causing infinite loops
        * in the graph.
        *
         */
        if self.marked_objs[index] {
            return;
        }
        // log only when the object is actually transitioning white -> gray
        #[cfg(feature = "debug-log-gc")]
        println!("{:?} mark {}", obj_ref, self.get(obj_ref).type_name());
        self.marked_objs[index] = true;
        self.gray_stack.push(obj_ref);
    }

    pub fn trace_references(&mut self) {
        while let Some(marked_ref) = self.gray_stack.pop() {
            // log when the object transitions gray -> black (references about to be traced)
            #[cfg(feature = "debug-log-gc")]
            println!("{:?} blacken {}", marked_ref, self.get(marked_ref).type_name());
            // Each object kind has different fields that might reference other
            // objects, therefore each type needs different handling.
            let refs_to_mark: Vec<ObjRef> = match self.get(marked_ref) {
                HeapObj::String(_) | HeapObj::NativeFunction(_) => vec![],
                // if the upvalue is closed, its value has been moved to the heap
                // and may itself reference another heap object
                HeapObj::Upvalue(upvalue) => {
                    upvalue.closed.and_then(|v| v.as_obj()).into_iter().collect()
                },
                HeapObj::Closure(closure) => {
                    std::iter::once(closure.obj_function)
                        .chain(closure.obj_upvalues.iter().copied())
                        .collect()
                },
                HeapObj::Function(function) => {
                    function.name.into_iter()
                        .chain(
                            function.chunk.get_constants().get_values().iter()
                                .filter_map(|v| v.as_obj())
                        )
                        .collect()
                },
            };

            for ref_to_mark in refs_to_mark {
                self.mark_object(ref_to_mark);
            }
        }
    }

    /*
    * After trace_references() has run, all objects on the heap
    * are either white or black.
    * Is time to free the memory occupied by the white objects which are
    * not reachable by the program.
     */
    pub fn sweep(&mut self) {
        for i in 0..self.marked_objs.len() {
            if self.marked_objs[i] {
                self.marked_objs[i] = false;
            } else {
                if self.objects[i].is_some() {
                    let size = size_of_val(&self.objects[i]);
                    self.bytes_allocated = self.bytes_allocated - size;
                    self.objects[i] = None;
                    self.free_slots.push(i)
                }
            }
        }
    }

    pub fn clear_strings(&mut self) {
        self.strings.retain(|_, obj_ref| self.marked_objs[obj_ref.0]);
    }
    
    pub fn intern_string(&mut self, s: String) -> ObjRef {
        if let Some(&handle) = self.strings.get(&s) {
            return handle;
        }
        let handle = self.alloc(HeapObj::String(ObjString { chars: s.clone() }));
        self.strings.insert(s, handle);
        handle
    }

}