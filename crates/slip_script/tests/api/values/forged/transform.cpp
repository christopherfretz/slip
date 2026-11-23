// spec: §5.2 rule 2
// A forged `info` (spec/api.md §5.2 rule 2): bits the host did not issue in
// this run, made with std::bit_cast of an integer, are refused with
// `invalid_handle` where a live value is required, and by every predicate
// of §6.2 (§4 rule 7, §6.2 rule 1); bits equal to a live value's are that
// value.
#include <slip/meta.h>

#include <bit>

// Whether the predicate `pred` of a forged value throws invalid_handle.
#define REFUSED(pred, value)                                                  \
  [&]() consteval {                                                           \
    try {                                                                     \
      (void)pred(value);                                                      \
      return false;                                                           \
    } catch (const error& e) {                                                \
      return e.code() == errc::invalid_handle;                                \
    }                                                                         \
  }()

using namespace slip;

consteval void say(bool b, std::string_view what) {
  report(severity::note, std::string(b ? "yes: " : "no: ") + std::string(what));
}

consteval void transform() {
  // Tag 1 (an entity, §3.4) and an index the host never issued.
  info forged = std::bit_cast<info>((1ull << 48) | 0xabcdefull);
  say(static_cast<bool>(forged), "the forged value is not null");
  try {
    (void)identifier_of(forged);
    say(false, "identifier_of(forged) throws");
  } catch (const error& e) {
    say(e.code() == errc::invalid_handle, "identifier_of(forged) throws invalid_handle");
  }
  try {
    (void)declarations_of(forged);
    say(false, "declarations_of(forged) throws");
  } catch (const error& e) {
    say(e.code() == errc::invalid_handle, "declarations_of(forged) throws invalid_handle");
  }
  say(REFUSED(is_entity, forged), "is_entity(forged) throws invalid_handle");
  say(REFUSED(is_declaration, forged), "is_declaration(forged) throws invalid_handle");
  say(REFUSED(is_reference, forged), "is_reference(forged) throws invalid_handle");
  say(REFUSED(is_expression, forged), "is_expression(forged) throws invalid_handle");
  say(REFUSED(is_namespace, forged), "is_namespace(forged) throws invalid_handle");
  say(REFUSED(is_type, forged), "is_type(forged) throws invalid_handle");
  say(REFUSED(is_function, forged), "is_function(forged) throws invalid_handle");
  say(REFUSED(is_variable, forged), "is_variable(forged) throws invalid_handle");
  say(REFUSED(is_template, forged), "is_template(forged) throws invalid_handle");
  say(REFUSED(is_parameter, forged), "is_parameter(forged) throws invalid_handle");
  say(REFUSED(is_definition, forged), "is_definition(forged) throws invalid_handle");
  say(REFUSED(is_call, forged), "is_call(forged) throws invalid_handle");
  // The null value is accepted: false, no throw.
  say(!REFUSED(is_entity, info{}) && !is_entity(info{}), "is_entity(null) is false");
  info f = lookup("f")[0];
  info copy = std::bit_cast<info>(std::bit_cast<unsigned long long>(f));
  say(copy == f, "the bits of a live value are that value");
  report(severity::note, "its identifier: " + identifier_of(copy));
}
