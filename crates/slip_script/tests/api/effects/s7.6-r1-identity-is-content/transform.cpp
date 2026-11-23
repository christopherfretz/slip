// spec: §7.6 rule 1
// Two requests with the same file, range and pieces are one edit; the
// output is a function of the set of requests, not of their order
// (spec/api.md §7.6 rule 1): the edits are requested from the last argument
// to the first and come out sorted by place; the edit of the second
// argument is requested three times, with code values made three ways
// (`copy(r)`, `cat(r)`, `copy(info)`), and is one edit.
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
  range first = extent_of(args[0]);
  replace(extent_of(args[2]), code{});
  replace(extent_of(args[1]), copy(first));
  replace(extent_of(args[1]), cat(first));
  replace(extent_of(args[1]), copy(args[0]));
  replace(first, code{});
}
