// spec: §7.3 rule 5
// `o.note` is shown beside the edit; it does not affect whether two
// requests are the same edit; an empty note is no note (spec/api.md §7.3
// rule 5). The first argument is deleted four times with the notes "b",
// "a", "" and "a": one edit with the notes "a" and "b" (§7.6: sorted,
// distinct). The second is deleted with an empty note: no note.
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
  replace(first, code{}, {.note = "b"});
  replace(first, code{}, {.note = "a"});
  replace(first, code{}, {.note = ""});
  replace(first, code{}, {.note = "a"});
  replace(extent_of(args[1]), code{}, {.note = ""});
}
