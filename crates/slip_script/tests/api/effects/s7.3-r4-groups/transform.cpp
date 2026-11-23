// spec: §7.3 rule 4
// `new_group()` returns a group with no member; `o.atomic`, if not the
// default, makes the edit a member of that group (spec/api.md §7.3 rule
// 4). Two groups of two edits each, one edit in no group, and a group that
// is made and never used (it has no member, so it is not in the output).
// Each new group is a different value, and the default is none of them.
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
  group g1 = new_group();
  group g2 = new_group();
  group unused = new_group();
  note(std::string("g1 == g1: ") + (g1 == g1 ? "true" : "false"));
  note(std::string("g1 == g2: ") + (g1 == g2 ? "true" : "false"));
  note(std::string("g2 == unused: ") + (g2 == unused ? "true" : "false"));
  note(std::string("g1 == group{}: ") + (g1 == group{} ? "true" : "false"));
  replace(extent_of(args[0]), code{}, {.atomic = g1});
  replace(extent_of(args[1]), code{}, {.atomic = g2});
  replace(extent_of(args[2]), code{}, {.atomic = g1});
  replace(extent_of(args[3]), code{}, {.atomic = g2});
  replace(extent_of(args[4]), code{}, {.atomic = group{}});
}
