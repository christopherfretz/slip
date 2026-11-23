// spec: §5.2 rule 2
// A forged `info` (spec/api.md §5.2 rule 2): bits the host did not issue in
// this run, made with std::bit_cast of an integer, are refused with
// `invalid_handle` where a live value is required; bits equal to a live
// value's are that value.
#include <slip/meta.h>

#include <bit>

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
  info f = lookup("f")[0];
  info copy = std::bit_cast<info>(std::bit_cast<unsigned long long>(f));
  say(copy == f, "the bits of a live value are that value");
  report(severity::note, "its identifier: " + identifier_of(copy));
}
