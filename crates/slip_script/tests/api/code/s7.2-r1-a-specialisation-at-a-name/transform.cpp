// spec: §7.2 rule 1
// A specialisation is not its own origin (spec/api.md §5.5): a name
// written for it is checked against its template (§7.2 rule 5,
// `spec/verify.md` §10.2 rule 4), never against it, so its spelling could
// not hold. `spell` of it is made (§7.1 rule 4), and at the name range of
// a reference, where the sibling conditions of §7.2 rule 1 hold
// (`tf<int>` has the parent of `tf`), `replace` throws `unsupported`
// naming it; at the extent too. The template itself is spelled there: its
// identifier, with its key.
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

// What a call did: "returned", or the message of the error it threw.
#define MESSAGE(...)                    \
  [&]() consteval -> std::string {      \
    try {                               \
      (void)(__VA_ARGS__);              \
      return "returned";                \
    } catch (const error& e) {          \
      return e.message();               \
    }                                   \
  }()

consteval void transform() {
  std::vector<info> rs = references_to(one("tf"));
  info spec = referent_of(rs[0]);  // tf<int>
  note("tf<int> at its name: " + OUTCOME(replace(name_range_of(rs[0]), spell(spec))));
  note("tf<int> at its extent: " + OUTCOME(replace(extent_of(rs[0]), spell(spec))));
  note("tf<int> at the name of tf(2L): " + OUTCOME(replace(name_range_of(rs[1]), spell(spec))));
  note("the message: " + MESSAGE(replace(name_range_of(rs[0]), spell(spec))));
  note("tf at its name: " + OUTCOME(replace(name_range_of(rs[0]), spell(one("tf")))));
}
