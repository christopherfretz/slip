// spec: §7.5 rule 2
// `expect_changed` throws unsupported (spec/api.md §7.5 rule 2): for an
// entity, a declaration, a call whose extent is null (the one `CALL`
// writes); for a name in the decl-specifiers shared by two declarators
// outside a body (`S a, b;`), in a friend declaration's decl-specifiers, in
// a block-scope `extern` declaration's type; for a name in a unit keyed by
// content (a `static_assert`, `spec/digest.md` §3.3). A null `info` is
// invalid_handle (§4). The reference in `k`'s initialiser is accepted.
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
  info g = one("g");
  std::vector<info> s = references_to(one("S"));
  note(num(s.size()) + " references to S");
  note("entity: " + OUTCOME(expect_changed(g, "w")));
  note("declaration: " + OUTCOME(expect_changed(declarations_of(g)[0], "w")));
  note("null extent: " + OUTCOME(expect_changed(calls_to(g)[0], "w")));
  note("null: " + OUTCOME(expect_changed(info{}, "w")));
  note("two declarators: " + OUTCOME(expect_changed(s[0], "w")));
  note("friend: " + OUTCOME(expect_changed(s[1], "w")));
  note("block-scope extern: " + OUTCOME(expect_changed(s[2], "w")));
  note("static_assert: " + OUTCOME(expect_changed(s[3], "w")));
  note("k: " + OUTCOME(expect_changed(calls_to(g)[1], "w")));
}
