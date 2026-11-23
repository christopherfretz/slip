// <slip/meta.h> -- layer L1 of the Slip transform API, version 0.
//
// A transform is a C++26 translation unit that includes this header and
// defines, in the global namespace,
//
//     consteval void transform();
//
// Slip compiles the transform once and evaluates `transform()` once for each
// subject translation unit. Every function below that is declared
// `consteval` asks the Slip host about that subject unit, or asks it to
// record something. Such a function may be evaluated only by `transform()`
// and what it calls: not in an initialiser at namespace scope, not in a
// `static_assert`, a template argument, an array bound, a `constexpr`
// variable's initialiser, an `if constexpr` condition or a constraint
// (spec/api.md §2.6).
//
// Normative text: spec/api.md. This file is the declaration of the names
// that specification defines; where they disagree the specification wins.

#ifndef SLIP_META_H
#define SLIP_META_H

#include <slip/__intrinsics.h>

#include <string>
#include <string_view>
#include <vector>

// Feature tests (spec/api.md §8.2). The value is the year and month of the
// last change to the feature's specification.
#define __slip_lib_meta 202611L           // values, entities, references, ranges
#define __slip_lib_meta_calls 202611L     // call expressions (provisional names)
#define __slip_lib_meta_code 202611L      // code values (provisional names)
#define __slip_lib_meta_edits 202611L     // replace, groups, reports, declared changes

namespace slip {
inline namespace v0 {

struct __access;

// ---------------------------------------------------------------------------
// Errors (spec/api.md §4).

// Why a query or an effect was refused. Open: later versions add values.
enum class errc : unsigned {
  invalid_handle = 1,    // a null, forged or foreign value where a live one is required
  wrong_kind = 2,        // a value of a sort the function does not accept
  invalid_argument = 3,  // text that is not what the function requires
  not_editable = 4,      // an edit to a range Slip may not edit
  unsupported = 5,       // well formed, and this version cannot answer it
  limit_exceeded = 6,    // more host-side objects than a run may make
};

// Thrown by every function below when it is misused. Absence is never an
// error: a query that finds nothing returns a null value or an empty
// sequence. Not derived from std::exception.
class error {
  errc __c;
  __l0::__u64 __n;
  constexpr error(errc __code, __l0::__u64 __ordinal) noexcept : __c(__code), __n(__ordinal) {}
  friend struct __access;

 public:
  constexpr errc code() const noexcept { return __c; }
  // What was wrong, for a person. The text is not specified.
  consteval std::string message() const;
};

// ---------------------------------------------------------------------------
// Values (spec/api.md §5).

// A piece of the subject translation unit: an entity, a declaration, a
// written reference to an entity, or an expression. Opaque. A
// default-constructed info is null and denotes nothing.
class info {
  __l0::__u64 __h = 0;
  constexpr explicit info(__l0::__u64 __handle) noexcept : __h(__handle) {}
  friend struct __access;

 public:
  constexpr info() noexcept = default;
  friend constexpr bool operator==(info, info) noexcept = default;
  constexpr explicit operator bool() const noexcept { return __h != 0; }
};

// A run of bytes of one source file, as the file is spelled. Opaque. A
// default-constructed range is null: "this has no place in a file that can
// be named as one range" (spec/api.md §5.4).
class range {
  unsigned __f = 0;
  unsigned __lo = 0;
  unsigned __hi = 0;
  friend struct __access;

 public:
  constexpr range() noexcept = default;
  friend constexpr bool operator==(range, range) noexcept = default;
  constexpr explicit operator bool() const noexcept { return __f != 0; }
};

// Replacement text under construction, held by the host. A
// default-constructed code is the empty text.
class code {
  __l0::__u64 __h = 0;
  constexpr explicit code(__l0::__u64 __handle) noexcept : __h(__handle) {}
  friend struct __access;

 public:
  constexpr code() noexcept = default;
};

// A set of edits that are applied together or not at all. A
// default-constructed group is "no group".
class group {
  __l0::__u64 __h = 0;
  constexpr explicit group(__l0::__u64 __handle) noexcept : __h(__handle) {}
  friend struct __access;

 public:
  constexpr group() noexcept = default;
  friend constexpr bool operator==(group, group) noexcept = default;
};

// ---------------------------------------------------------------------------
// Sorts and kinds (spec/api.md §6.2). False for a null info.

consteval bool is_entity(info);
consteval bool is_declaration(info);
consteval bool is_reference(info);
consteval bool is_expression(info);

consteval bool is_namespace(info __entity);
consteval bool is_type(info __entity);       // class, enumeration or alias
consteval bool is_function(info __entity);   // not a function template
consteval bool is_variable(info __entity);   // not a parameter
consteval bool is_template(info __entity);
consteval bool is_parameter(info __entity);

consteval bool is_definition(info __declaration);
consteval bool is_call(info __expression);

// ---------------------------------------------------------------------------
// Entities and declarations (spec/api.md §6.1, §6.3, §6.4).

// The entities declared with this qualified name in the subject unit, in
// unit order. Matching is by declared name, not by C++ name lookup: no
// using-declaration, using-directive or alias is followed. Empty if none.
consteval std::vector<info> lookup(std::string_view __qualified_name);

// The identifier an entity is declared with, or that a reference is written
// with. Empty for an entity with no identifier (an operator, a constructor,
// an unnamed entity).
consteval std::string identifier_of(info);

// A description for a person. The text is not specified and must not be
// parsed.
consteval std::string display_string_of(info);

// The entity whose scope an entity is a member of. Null for the global
// namespace.
consteval info parent_of(info __entity);

// The parameters of a function (an entity or one declaration of it), in
// order. PROVISIONAL for an entity (spec/api.md §8.4).
consteval std::vector<info> parameters_of(info __function);

// The declarations of an entity that are written in the subject unit, in
// unit order.
consteval std::vector<info> declarations_of(info __entity);

// The entity a declaration declares.
consteval info entity_of(info __declaration);

// ---------------------------------------------------------------------------
// References and ranges (spec/api.md §6.5, §6.6).

// The written references to an entity in the subject unit, in unit order.
// A reference to a specialisation of a template is a reference to the
// template.
consteval std::vector<info> references_to(info __entity);

// The entity a written reference denotes, after overload resolution.
consteval info referent_of(info __reference);

// The tokens of the name itself: of a declaration's own name, or of a
// reference without its qualifier. Null if they are not one range.
consteval range name_range_of(info __declaration_or_reference);

// Everything a reference or an expression is written with: for a reference
// its qualifier and its name; for an expression, its first token to its
// last. Null if that is not one range.
consteval range extent_of(info __reference_or_expression);

// The bytes of a range.
consteval std::string source_text_of(range);

// Whether Slip may edit this range. False for a null range.
consteval bool is_editable(range);

// ---------------------------------------------------------------------------
// Calls (spec/api.md §6.7). PROVISIONAL names: __slip_lib_meta_calls.

// The written call expressions whose callee is a name that denotes this
// function, or a specialisation of this function template; in unit order.
consteval std::vector<info> calls_to(info __function);

// The function a call calls, after overload resolution.
consteval info callee_of(info __call);

// The reference that names the callee of a call.
consteval info callee_reference_of(info __call);

// The argument expressions between the parentheses of a call, as written:
// no defaulted argument, no object expression.
consteval std::vector<info> arguments_as_written(info __call);

// ---------------------------------------------------------------------------
// Code values (spec/api.md §7.1). PROVISIONAL names: __slip_lib_meta_code.

// This literal text.
consteval code text(std::string_view);

// A name that denotes this entity at the place the code is put. Slip chooses
// the spelling, and verification checks that it denotes this entity.
consteval code spell(info __entity);

// The text of a range, with every edit that lies inside the range applied.
consteval code copy(range);

// copy(extent_of(x)).
consteval code copy(info __reference_or_expression);

// The pieces in order. A piece is a code, a range (copied) or anything
// convertible to std::string_view (literal text).
template <class... _Pieces>
consteval code cat(const _Pieces&... __pieces);

// ---------------------------------------------------------------------------
// Effects (spec/api.md §7.2 to §7.5). An effect records a request. Nothing a
// transform can query changes.

// Initialise with designated initialisers only: members are added over time.
struct edit_options {
  group atomic = {};           // apply all edits of the group, or none
  std::string_view note = {};  // shown beside the edit in reports
};

// A new, empty group.
consteval group new_group();

// Asks that the bytes of the range be replaced by the code.
consteval void replace(range, code, edit_options = {});

// Open: later versions add values.
enum class severity : unsigned {
  note = 0,    // information
  review = 1,  // a person should look at this place
  error = 2,   // the transform could not do its job here
};

consteval void report(severity, std::string_view __message);
consteval void report(info __where, severity, std::string_view __message);
consteval void report(range __where, severity, std::string_view __message);

// Declares that the meaning of the declaration enclosing this place is
// intended to change. Verification then does not compare it, and says so.
// The place is a reference or an expression with an extent; the
// declaration is in force only if an edit at that place is accepted. An
// entity or a declaration is refused (unsupported): version 0 has no
// declared change without a place.
consteval void expect_changed(info __reference_or_expression, std::string_view __why);

// Declares that this entity is intended to keep everything but its
// identifier. Verification then compares it under the new name.
consteval void expect_renamed(info __entity, std::string_view __new_identifier);

// ===========================================================================
// Implementation. Nothing below is part of the contract.

struct __access {
  static constexpr info __info(__l0::__u64 __h) noexcept { return info(__h); }
  static constexpr code __code(__l0::__u64 __h) noexcept { return code(__h); }
  static constexpr group __group(__l0::__u64 __h) noexcept { return group(__h); }
  static constexpr range __range(__l0::__u64 __file, __l0::__u64 __span) noexcept {
    range __r;
    __r.__f = static_cast<unsigned>(__file);
    __r.__lo = static_cast<unsigned>(__span >> 32);
    __r.__hi = static_cast<unsigned>(__span & 0xffffffffull);
    return __r;
  }
  static constexpr __l0::__u64 __raw(info __i) noexcept { return __i.__h; }
  static constexpr __l0::__u64 __raw(code __c) noexcept { return __c.__h; }
  static constexpr __l0::__u64 __raw(group __g) noexcept { return __g.__h; }
  static constexpr __l0::__u64 __file(range __r) noexcept { return __r.__f; }
  static constexpr __l0::__u64 __span(range __r) noexcept {
    return (static_cast<__l0::__u64>(__r.__lo) << 32) | __r.__hi;
  }
  static constexpr __l0::__u64 __ordinal(const error& __e) noexcept { return __e.__n; }
  // The result word of a call that did not fail; throws for one that did.
  static constexpr __l0::__u64 __ok(__l0::__u64 __r) {
    if (__l0::__status(__r) != 0)
      throw error(static_cast<errc>(__l0::__status(__r)), __l0::__value(__r));
    return __r;
  }
};

namespace __impl {

using __l0::__u64;

// A call with integer arguments and optional text, result convention V.
consteval __u64 __v(unsigned __op, const __u64* __in, unsigned __n_in,
                    std::string_view __bin = {}) {
  return __l0::__value(__access::__ok(
      __l0::__call(__op, __in, __n_in, __bin.data(), static_cast<unsigned>(__bin.size()),
                   nullptr, 0, nullptr, 0)));
}

consteval __u64 __v1(unsigned __op, __u64 __a, std::string_view __bin = {}) {
  __u64 __in[1] = {__a};
  return __v(__op, __in, 1, __bin);
}

// Result convention B, integers: a sequence of info.
consteval std::vector<info> __infos(unsigned __op, const __u64* __in, unsigned __n_in,
                                    std::string_view __bin = {}) {
  const unsigned __n_bin = static_cast<unsigned>(__bin.size());
  __u64 __small[16];
  __u64 __r = __access::__ok(
      __l0::__call(__op, __in, __n_in, __bin.data(), __n_bin, __small, 16, nullptr, 0));
  const unsigned __n = __l0::__n_ints(__r);
  std::vector<info> __result;
  __result.reserve(__n);
  if (__n <= 16) {
    for (unsigned __i = 0; __i != __n; ++__i) __result.push_back(__access::__info(__small[__i]));
    return __result;
  }
  std::vector<__u64> __big(__n);
  __access::__ok(
      __l0::__call(__op, __in, __n_in, __bin.data(), __n_bin, __big.data(), __n, nullptr, 0));
  for (unsigned __i = 0; __i != __n; ++__i) __result.push_back(__access::__info(__big[__i]));
  return __result;
}

consteval std::vector<info> __infos1(unsigned __op, info __a) {
  __u64 __in[1] = {__access::__raw(__a)};
  return __infos(__op, __in, 1);
}

// Result convention B, bytes: text.
consteval std::string __string(unsigned __op, const __u64* __in, unsigned __n_in) {
  char __small[64];
  __u64 __r = __access::__ok(
      __l0::__call(__op, __in, __n_in, nullptr, 0, nullptr, 0, __small, 64));
  const unsigned __n = __l0::__n_bytes(__r);
  if (__n <= 64) return std::string(__small, __n);
  std::string __result(__n, '\0');
  __access::__ok(
      __l0::__call(__op, __in, __n_in, nullptr, 0, nullptr, 0, __result.data(), __n));
  return __result;
}

// Result convention B, two integers or none: a range.
consteval range __range1(unsigned __op, info __a) {
  __u64 __in[1] = {__access::__raw(__a)};
  __u64 __out[2];
  __u64 __r =
      __access::__ok(__l0::__call(__op, __in, 1, nullptr, 0, __out, 2, nullptr, 0));
  if (__l0::__n_ints(__r) != 2) return range();
  return __access::__range(__out[0], __out[1]);
}

consteval __u64 __kind(info __i) {
  if (!__i) return 0;
  return __v1(__l0::__op_kind, __access::__raw(__i));
}

consteval info __info1(unsigned __op, info __a) {
  return __access::__info(__v1(__op, __access::__raw(__a)));
}

consteval code __piece(code __c) { return __c; }
consteval code __piece(range __r) { return copy(__r); }
consteval code __piece(std::string_view __s) { return text(__s); }

}  // namespace __impl

consteval std::string error::message() const {
  __l0::__u64 __in[1] = {__n};
  return __impl::__string(__l0::__op_error_message, __in, 1);
}

consteval bool is_entity(info __i) { return (__impl::__kind(__i) & __l0::__kind_entity) != 0; }
consteval bool is_declaration(info __i) {
  return (__impl::__kind(__i) & __l0::__kind_declaration) != 0;
}
consteval bool is_reference(info __i) {
  return (__impl::__kind(__i) & __l0::__kind_reference) != 0;
}
consteval bool is_expression(info __i) {
  return (__impl::__kind(__i) & __l0::__kind_expression) != 0;
}
consteval bool is_namespace(info __i) {
  return (__impl::__kind(__i) & __l0::__kind_namespace) != 0;
}
consteval bool is_type(info __i) { return (__impl::__kind(__i) & __l0::__kind_type) != 0; }
consteval bool is_function(info __i) {
  return (__impl::__kind(__i) & __l0::__kind_function) != 0;
}
consteval bool is_variable(info __i) {
  return (__impl::__kind(__i) & __l0::__kind_variable) != 0;
}
consteval bool is_template(info __i) {
  return (__impl::__kind(__i) & __l0::__kind_template) != 0;
}
consteval bool is_parameter(info __i) {
  return (__impl::__kind(__i) & __l0::__kind_parameter) != 0;
}
consteval bool is_definition(info __i) {
  return (__impl::__kind(__i) & __l0::__kind_definition) != 0;
}
consteval bool is_call(info __i) { return (__impl::__kind(__i) & __l0::__kind_call) != 0; }

consteval std::vector<info> lookup(std::string_view __qualified_name) {
  return __impl::__infos(__l0::__op_lookup, nullptr, 0, __qualified_name);
}

consteval std::string identifier_of(info __i) {
  __l0::__u64 __in[1] = {__access::__raw(__i)};
  return __impl::__string(__l0::__op_identifier_of, __in, 1);
}

consteval std::string display_string_of(info __i) {
  __l0::__u64 __in[1] = {__access::__raw(__i)};
  return __impl::__string(__l0::__op_display_string_of, __in, 1);
}

consteval info parent_of(info __entity) {
  return __impl::__info1(__l0::__op_parent_of, __entity);
}

consteval std::vector<info> parameters_of(info __function) {
  return __impl::__infos1(__l0::__op_parameters_of, __function);
}

consteval std::vector<info> declarations_of(info __entity) {
  return __impl::__infos1(__l0::__op_declarations_of, __entity);
}

consteval info entity_of(info __declaration) {
  return __impl::__info1(__l0::__op_entity_of, __declaration);
}

consteval std::vector<info> references_to(info __entity) {
  return __impl::__infos1(__l0::__op_references_to, __entity);
}

consteval info referent_of(info __reference) {
  return __impl::__info1(__l0::__op_referent_of, __reference);
}

consteval range name_range_of(info __i) {
  return __impl::__range1(__l0::__op_name_range_of, __i);
}

consteval range extent_of(info __i) { return __impl::__range1(__l0::__op_extent_of, __i); }

consteval std::string source_text_of(range __r) {
  __l0::__u64 __in[2] = {__access::__file(__r), __access::__span(__r)};
  return __impl::__string(__l0::__op_source_text_of, __in, 2);
}

consteval bool is_editable(range __r) {
  if (!__r) return false;
  __l0::__u64 __in[2] = {__access::__file(__r), __access::__span(__r)};
  return __impl::__v(__l0::__op_is_editable, __in, 2) != 0;
}

consteval std::vector<info> calls_to(info __function) {
  return __impl::__infos1(__l0::__op_calls_to, __function);
}

consteval info callee_of(info __call) { return __impl::__info1(__l0::__op_callee_of, __call); }

consteval info callee_reference_of(info __call) {
  return __impl::__info1(__l0::__op_callee_reference_of, __call);
}

consteval std::vector<info> arguments_as_written(info __call) {
  return __impl::__infos1(__l0::__op_arguments_as_written, __call);
}

consteval code text(std::string_view __s) {
  return __access::__code(__impl::__v(__l0::__op_code_text, nullptr, 0, __s));
}

consteval code spell(info __entity) {
  return __access::__code(__impl::__v1(__l0::__op_code_spell, __access::__raw(__entity)));
}

consteval code copy(range __r) {
  __l0::__u64 __in[2] = {__access::__file(__r), __access::__span(__r)};
  return __access::__code(__impl::__v(__l0::__op_code_copy, __in, 2));
}

consteval code copy(info __i) { return copy(extent_of(__i)); }

template <class... _Pieces>
consteval code cat(const _Pieces&... __pieces) {
  // One more element than pieces, so that the array is never empty.
  __l0::__u64 __in[sizeof...(_Pieces) + 1] = {__access::__raw(__impl::__piece(__pieces))...};
  return __access::__code(__impl::__v(__l0::__op_code_cat, __in, sizeof...(_Pieces)));
}

consteval group new_group() {
  return __access::__group(__impl::__v(__l0::__op_new_group, nullptr, 0));
}

consteval void replace(range __r, code __c, edit_options __o) {
  __l0::__u64 __in[4] = {__access::__file(__r), __access::__span(__r), __access::__raw(__c),
                         __access::__raw(__o.atomic)};
  __impl::__v(__l0::__op_replace, __in, 4, __o.note);
}

consteval void report(severity __s, std::string_view __message) {
  __l0::__u64 __in[4] = {0, 0, 0, static_cast<__l0::__u64>(__s)};
  __impl::__v(__l0::__op_report, __in, 4, __message);
}

consteval void report(info __where, severity __s, std::string_view __message) {
  __l0::__u64 __in[4] = {1, __access::__raw(__where), 0, static_cast<__l0::__u64>(__s)};
  __impl::__v(__l0::__op_report, __in, 4, __message);
}

consteval void report(range __where, severity __s, std::string_view __message) {
  __l0::__u64 __in[4] = {2, __access::__file(__where), __access::__span(__where),
                         static_cast<__l0::__u64>(__s)};
  __impl::__v(__l0::__op_report, __in, 4, __message);
}

consteval void expect_changed(info __reference_or_expression, std::string_view __why) {
  __impl::__v1(__l0::__op_expect_changed, __access::__raw(__reference_or_expression), __why);
}

consteval void expect_renamed(info __entity, std::string_view __new_identifier) {
  __impl::__v1(__l0::__op_expect_renamed, __access::__raw(__entity), __new_identifier);
}

}  // namespace v0
}  // namespace slip

#endif  // SLIP_META_H
