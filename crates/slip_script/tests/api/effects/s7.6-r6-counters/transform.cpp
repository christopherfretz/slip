// spec: §7.6 rule 6
// counters: api.edits, api.reports, api.unresolved_spelling_matches, api.one_shot_key_servers, api.op.lookup, api.op.code_text, api.op.replace, api.op.report, api.op.new_group, api.op.expect_renamed
// The counters of a run (spec/api.md §7.6 rule 6): `api.edits` and
// `api.reports` (two distinct edits, two distinct reports, none requested
// twice), `api.unresolved_spelling_matches` (0: every name resolves),
// `api.one_shot_key_servers` only when it is not 0 (absent), and one
// `api.op.<name>` (§3.2's names) per operation that succeeded at least once,
// counting its calls that succeeded: `lookup` once, `code_text` twice,
// `replace` twice (a third call fails), `report` twice; `new_group` was
// never called and `expect_renamed` never succeeded (absent). The order of
// the counters is the output's (§11 rule 3), which the text does not fix:
// this expectation has the host's (by name).
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
  info f = lookup("f")[0];
  (void)OUTCOME(expect_renamed(declarations_of(f)[0], "g"));
  std::vector<info> args = arguments_as_written(calls_to(f)[0]);
  replace(extent_of(args[0]), text("a"));
  replace(extent_of(args[1]), text("b"));
  (void)OUTCOME(replace(range{}, code{}));
  report(severity::note, "one");
  report(severity::note, "two");
}
