// spec: §7.1 rule 5
// `cat` concatenates (spec/api.md §7.1 rule 5). A piece is a `code`, a
// `range` (as by `copy`) or a value convertible to `std::string_view` (as by
// `text`): here a string literal, a `std::string`, a `std::string_view`, a
// range, a copy and a spelled entity, no two literals adjacent. The
// literals are markers, not C++.
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
  info call = calls_to(one("f"))[0];
  std::vector<info> args = arguments_as_written(call);
  code c = cat("<", extent_of(args[1]), std::string("|"), copy(args[0]), std::string_view(">"),
               spell(one("g")));
  replace(extent_of(call), c);
}
