// spec: §7.5 rule 3
// `expect_renamed(e, n)` and `identifier(n)` take the same identifiers
// (spec/api.md §7.5 rule 3, §7.1 rule 7): each text that one refuses the
// other refuses with the same `errc`, and the name the rename declares is
// the name the identifier piece writes.
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

consteval void both(std::string_view what, std::string_view n) {
  note(std::string(what) + ": identifier " + OUTCOME(identifier(n)) + ", expect_renamed " +
       OUTCOME(expect_renamed(one("f"), n)));
}

consteval void transform() {
  both("keyword", "class");
  both("two tokens", "a b");
  both("empty", "");
  both("qualified", "n::x");
  both("not UTF-8", "\xff");
  both("dollar", "a$");
  both("plain", "g");
  replace(name_range_of(declarations_of(one("f"))[0]), identifier("g"));
}
