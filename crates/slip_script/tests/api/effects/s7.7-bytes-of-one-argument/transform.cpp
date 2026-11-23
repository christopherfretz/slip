// spec: §7.7
// Two rows of the limits (spec/api.md §7.7): "bytes in one note, reason or
// message" and "bytes in one qualified name or identifier argument", both
// 2^12. Holding exactly a limit does not exceed it; one byte more throws
// limit_exceeded and records nothing (§4 rule 3). The message, note and
// reason of 4096 bytes are recorded; the name of 4096 bytes is looked up
// and not found; the identifier of 4096 bytes is a declared rename.
#include <slip/meta.h>

using namespace slip;

consteval std::string num(unsigned long long n) {
  return n < 10 ? std::string(1, static_cast<char>('0' + n))
                : num(n / 10) + static_cast<char>('0' + n % 10);
}

consteval std::string errc_name(errc c) {
  switch (c) {
    case errc::invalid_handle: return "invalid_handle";
    case errc::wrong_kind: return "wrong_kind";
    case errc::invalid_argument: return "invalid_argument";
    case errc::not_editable: return "not_editable";
    case errc::unsupported: return "unsupported";
    case errc::limit_exceeded: return "limit_exceeded";
  }
  return "errc " + num(static_cast<unsigned>(c));
}

// What a call did: "returned", or the name of the errc it threw.
#define OUTCOME(...)                    \
  [&]() consteval -> std::string {      \
    try {                               \
      (void)(__VA_ARGS__);              \
      return "returned";                \
    } catch (const error& e) {          \
      return errc_name(e.code());       \
    }                                   \
  }()

consteval void note(std::string_view s) { report(severity::note, s); }

consteval info one(std::string_view name) { return lookup(name)[0]; }

consteval void transform() {
  info f = one("n::f");
  info r = references_to(f)[0];
  range name = name_range_of(r);
  std::string at(4096, 'm');
  std::string over(4097, 'm');
  std::string id_at = "n::" + std::string(4093, 'k');
  std::string id_over = "n::" + std::string(4094, 'k');
  note("message 4096: " + OUTCOME(report(severity::note, at)));
  note("message 4097: " + OUTCOME(report(severity::note, over)));
  note("note 4096: " + OUTCOME(replace(name, code{}, {.note = at})));
  note("note 4097: " + OUTCOME(replace(extent_of(r), code{}, {.note = over})));
  note("reason 4096: " + OUTCOME(expect_changed(r, at)));
  note("reason 4097: " + OUTCOME(expect_changed(calls_to(f)[0], over)));
  note("qualified name 4096: " + OUTCOME(lookup(id_at)));
  note("qualified name 4097: " + OUTCOME(lookup(id_over)));
  note("identifier 4097: " + OUTCOME(expect_renamed(f, std::string(4097, 'k'))));
  note("identifier 4096: " + OUTCOME(expect_renamed(f, std::string(4096, 'k'))));
}
