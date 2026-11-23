// spec: §7.3 rule 2
// What `replace` refuses (spec/api.md §7.3 rule 2): a null range
// (invalid_handle); a range that neither `name_range_of` nor `extent_of`
// returns in the subject unit, made by changing the bytes of a live range
// (invalid_handle, §5.2 rule 2): the name `ff` without its first byte, and
// the run from the call's first byte to the end of the line; a forged code
// value and a forged group (invalid_handle, §4: a `code` or `group` "not
// produced by a host function in this run"). The same bits as a live range
// are that range (§5.2 rule 2), and are accepted. Each refusal records
// nothing (§4 rule 3): the one edit is the last request.
//
// Not tested: an empty range made the same way (`lo == hi`), which is both
// "an empty range" (invalid_argument) and a range no query returns
// (invalid_handle); the text does not say which is thrown.
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

#include <bit>

struct raw_range {
  unsigned file, lo, hi;
};

consteval void transform() {
  info call = calls_to(one("ff"))[0];
  range name = name_range_of(declarations_of(one("ff"))[0]);
  range whole = extent_of(call);
  raw_range n = std::bit_cast<raw_range>(name);
  raw_range w = std::bit_cast<raw_range>(whole);
  range shorter = std::bit_cast<range>(raw_range{n.file, n.lo + 1, n.hi});
  range longer = std::bit_cast<range>(raw_range{w.file, w.lo, w.hi + 1});
  range same = std::bit_cast<range>(n);
  note("null range: " + OUTCOME(replace(range{}, code{})));
  note("the name without its first byte: " + OUTCOME(replace(shorter, code{})));
  note("the call and the `;` after it: " + OUTCOME(replace(longer, code{})));
  note("a forged code: " + OUTCOME(replace(name, std::bit_cast<code>((5ull << 48) | 0xabcdull))));
  note("a forged group: " +
       OUTCOME(replace(name, code{}, {.atomic = std::bit_cast<group>((6ull << 48) | 0xabcdull)})));
  note("the bits of a live range: " + OUTCOME(replace(same, copy(call))));
}
