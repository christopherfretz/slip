// <slip/__intrinsics.h> -- layer L0 of the Slip transform API.
//
// PRIVATE. Nothing here is part of the contract of <slip/meta.h>: names,
// operation numbers and encodings may change in any Slip release. A Slip
// host accepts only the copy of this file it was built with
// (spec/api.md §3.6), so a transform cannot depend on it by accident.
//
// Specified by spec/api.md §3.

#ifndef SLIP___INTRINSICS_H
#define SLIP___INTRINSICS_H

// 1 inside a Slip transform unit, where the host call exists; 0 under any
// other compiler, where this header still parses (so that editors and
// ordinary compilers can check a transform) and nothing in it can run.
#if defined(__has_builtin)
#  if __has_builtin(__builtin_kiln_host_call)
#    define __SLIP_HOSTED 1
#  endif
#endif
#ifndef __SLIP_HOSTED
#  define __SLIP_HOSTED 0
#endif

namespace slip::__l0 {

using __u64 = unsigned long long;

// The one primitive (spec/api.md §3.1).
#if __SLIP_HOSTED
constexpr __u64 __call(unsigned __op, const __u64* __in, unsigned __n_in,
                       const char* __bin, unsigned __n_bin, __u64* __out,
                       unsigned __cap_out, char* __bout, unsigned __cap_bout) {
  return __builtin_kiln_host_call(__op, __in, __n_in, __bin, __n_bin, __out,
                                  __cap_out, __bout, __cap_bout);
}
#else
constexpr __u64 __call(unsigned __op, const __u64* __in, unsigned __n_in,
                       const char* __bin, unsigned __n_bin, __u64* __out,
                       unsigned __cap_out, char* __bout, unsigned __cap_bout);
#endif

// The result word (spec/api.md §3.3).
//   status != 0:  status << 56 | ordinal of the failure in this run
//   convention V: value                     (value < 2^56)
//   convention B: n_bytes << 28 | n_ints    (each < 2^28)
constexpr unsigned __status(__u64 __r) { return static_cast<unsigned>(__r >> 56); }
constexpr __u64 __value(__u64 __r) { return __r & 0x00ffffffffffffffull; }
constexpr unsigned __n_ints(__u64 __r) { return static_cast<unsigned>(__r & 0x0fffffffull); }
constexpr unsigned __n_bytes(__u64 __r) {
  return static_cast<unsigned>((__r >> 28) & 0x0fffffffull);
}

// Operation numbers (spec/api.md §3.2). A number is never reused for a
// different operation, in any release.
enum : unsigned {
  __op_error_message = 2,

  __op_lookup = 16,
  __op_kind = 17,
  __op_identifier_of = 18,
  __op_display_string_of = 19,
  __op_parent_of = 20,
  __op_parameters_of = 21,
  __op_declarations_of = 22,
  __op_entity_of = 23,
  __op_lookup_in = 24,

  __op_references_to = 32,
  __op_referent_of = 33,
  __op_name_range_of = 34,
  __op_extent_of = 35,
  __op_source_text_of = 36,
  __op_is_editable = 37,

  __op_calls_to = 48,
  __op_callee_of = 49,
  __op_callee_reference_of = 50,
  __op_arguments_as_written = 51,

  __op_code_text = 64,
  __op_code_spell = 65,
  __op_code_copy = 66,
  __op_code_cat = 67,
  __op_code_identifier = 68,

  __op_replace = 80,
  __op_new_group = 81,
  __op_report = 82,
  __op_expect_changed = 83,
  __op_expect_renamed = 84,
};

// Bits of the value of __op_kind (spec/api.md §3.4).
enum : __u64 {
  __kind_entity = 1ull << 0,
  __kind_declaration = 1ull << 1,
  __kind_reference = 1ull << 2,
  __kind_expression = 1ull << 3,

  __kind_namespace = 1ull << 8,
  __kind_type = 1ull << 9,
  __kind_function = 1ull << 10,
  __kind_variable = 1ull << 11,
  __kind_template = 1ull << 12,
  __kind_parameter = 1ull << 13,

  __kind_definition = 1ull << 16,
  __kind_call = 1ull << 17,
};

}  // namespace slip::__l0

#endif  // SLIP___INTRINSICS_H
