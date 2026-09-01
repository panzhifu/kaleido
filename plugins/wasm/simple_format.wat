;; WASM Format Plugin - Simple Format Decoder/Encoder
;; Exports: format_decode, format_encode, alloc, free
;; Imports: host_alloc, host_free, host_log

(module
  ;; Import host functions
  (import "host" "alloc" (func $host_alloc (param i32) (result i32)))
  (import "host" "free" (func $host_free (param i32 param i32)))
  (import "host" "log" (func $host_log (param i32 param i32)))

  ;; Memory
  (memory (export "memory") 1)

  ;; Global buffer pointer
  (global $buffer_ptr (mut i32) (i32.const 1024))

  ;; alloc - allocate memory in WASM
  (func $alloc (export "alloc") (param $size i32) (result i32)
    (local $ptr i32)
    ;; Simple bump allocator
    (local.set $ptr (global.get $buffer_ptr))
    (global.set $buffer_ptr
      (i32.add (global.get $buffer_ptr) (local.get $size)))
    (local.get $ptr)
  )

  ;; free - no-op for bump allocator
  (func $free (export "free") (param $ptr i32) (param $size i32)
    ;; No-op
  )

  ;; format_decode - decode an image file
  ;; Returns: i64 (buffer handle: high=ptr, low=len)
  (func $format_decode (export "format_decode")
    (param $path_ptr i32)
    (param $path_len i32)
    (result i64)
    (local $ptr i32)
    (local $data_ptr i32)

    ;; Allocate buffer for decoded data (4 bytes for RGBA pixel)
    (local.set $ptr (call $alloc (i32.const 16)))

    ;; Write simple RGBA pixel data (red pixel: 255, 0, 0, 255)
    (local.set $data_ptr (local.get $ptr))
    (i32.store8 (i32.add (local.get $data_ptr) (i32.const 0)) (i32.const 255)) ;; R
    (i32.store8 (i32.add (local.get $data_ptr) (i32.const 1)) (i32.const 0))   ;; G
    (i32.store8 (i32.add (local.get $data_ptr) (i32.const 2)) (i32.const 0))   ;; B
    (i32.store8 (i32.add (local.get $data_ptr) (i32.const 3)) (i32.const 255)) ;; A

    ;; Return handle: high=ptr, low=len(4)
    (i64.or
      (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32))
      (i64.extend_i32_u (i32.const 4))
    )
  )

  ;; format_encode - encode image data to file
  ;; Returns: i32 (0=success, -1=error)
  (func $format_encode (export "format_encode")
    (param $path_ptr i32)
    (param $path_len i32)
    (param $data_ptr i32)
    (param $data_len i32)
    (param $width i32)
    (param $height i32)
    (result i32)
    ;; Success
    (i32.const 0)
  )

  ;; plugin_init - initialization
  (func $plugin_init (export "plugin_init")
    ;; Log initialization message
    (call $host_log (i32.const 0) (i32.const 0))
  )

  ;; plugin_manifest_json - returns manifest JSON string pointer
  (func $plugin_manifest_json (export "plugin_manifest_json") (result i32)
    (i32.const 0) ;; placeholder
  )

  ;; format_get_name - returns format name string pointer
  (func $format_get_name (export "format_get_name") (result i32)
    (i32.const 0) ;; placeholder
  )

  ;; format_get_exts - returns supported extensions string pointer
  (func $format_get_exts (export "format_get_exts") (result i32)
    (i32.const 0) ;; placeholder
  )
)
