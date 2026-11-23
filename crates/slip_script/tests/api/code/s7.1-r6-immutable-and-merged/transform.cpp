// spec: §7.1 rule 6
// Code values are immutable, adjacent literal pieces are one piece, an
// empty literal is no piece, and two code values with the same pieces are
// interchangeable (spec/api.md §7.1 rule 6). `c` is used after `cat(c, ...)`
// made another value from it and is unchanged; `cat("a", "b")` is one
// literal; `cat("", copy(r), "")` is the copy alone; `cat(text(""),
// text(""))` is empty (deletion); `cat("ab")` and `cat(text("a"),
// text("b"))` replace one range and are one edit (§7.6 rule 1).
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
  std::vector<info> args = arguments_as_written(calls_to(one("f"))[0]);
  code c = text("a");
  code d = cat(c, "b");
  replace(extent_of(args[0]), c);
  replace(extent_of(args[1]), cat("", extent_of(args[3]), ""));
  replace(extent_of(args[2]), cat(text(""), text("")));
  replace(extent_of(args[3]), d);
  replace(extent_of(args[3]), cat("ab"));
  replace(extent_of(args[3]), cat(text("a"), text("b")));
}
