# Foreign Function Interface (FFI)

FFI is a way to call functions from other languages into Phoenix.

Example: 
```
@extern malloc :: (size: u8) -> *mut u8; // C malloc returns a pointer to the allocated memory
```

```
@extern free :: (ptr: *mut u8) -> (); // C free frees the memory pointed to by the pointer
```