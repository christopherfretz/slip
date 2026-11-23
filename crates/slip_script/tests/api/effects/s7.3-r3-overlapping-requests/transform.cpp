// spec: §7.3 rule 3
// The host does not check that requests are compatible: two requests for
// overlapping ranges are both recorded (spec/api.md §7.3 rule 3). The call
// `n::f(n::g(1), 2)`, its first argument, the callee of `n::g(1)` (its
// whole extent, qualifier included) and its name `g` are each replaced;
// each range lies inside the one before.
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
  info outer = calls_to(one("n::f"))[0];
  info inner = calls_to(one("n::g"))[0];
  info callee = callee_reference_of(inner);
  replace(extent_of(outer), copy(arguments_as_written(outer)[1]));
  replace(extent_of(arguments_as_written(outer)[0]), copy(arguments_as_written(inner)[0]));
  replace(extent_of(callee), copy(arguments_as_written(outer)[1]));
  replace(name_range_of(callee), code{});
}
