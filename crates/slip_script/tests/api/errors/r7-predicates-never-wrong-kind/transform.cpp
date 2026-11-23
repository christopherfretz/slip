// spec: §4 rule 7
// The predicates of §6.2 accept every `info` the host gave, of any kind,
// and the null `info`, and never throw `wrong_kind` (spec/api.md §4 rule 7):
// each of the twelve, of an entity, a declaration, a reference, a call, an
// argument and the null value, answers. The forged case is in
// `values/forged`.
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

// What a call did: "returned", or the errc it threw, its value (§4: the
// enumerators' values are part of the API), and whether it has a message.
#define OUTCOME(...)                                                          \
  [&]() consteval -> std::string {                                            \
    try {                                                                     \
      (void)(__VA_ARGS__);                                                    \
      return "returned";                                                      \
    } catch (const error& e) {                                                \
      return errc_name(e.code()) + " = " + num(static_cast<unsigned>(e.code())) + \
             (e.message().empty() ? ", no message" : ", a message");           \
    }                                                                         \
  }()


#define ALL(F)                                                              \
  F(is_entity) F(is_declaration) F(is_reference) F(is_expression) F(is_namespace) F(is_type) \
  F(is_function) F(is_variable) F(is_template) F(is_parameter) F(is_definition) F(is_call)

consteval void check(std::string_view what, info i) {
  unsigned answered = 0, threw = 0;
#define ONE(pred)                                                             \
  try {                                                                       \
    (void)pred(i);                                                            \
    ++answered;                                                               \
  } catch (const error&) {                                                    \
    ++threw;                                                                  \
  }
  ALL(ONE)
#undef ONE
  report(severity::note, std::string(what) + ": " + num(answered) + " answered, " + num(threw) +
                             " threw");
}

consteval void transform() {
  info f = lookup("f")[0];
  info call = calls_to(f)[0];
  check("an entity", f);
  check("a declaration", declarations_of(f)[0]);
  check("a reference", references_to(f)[0]);
  check("a call", call);
  check("an argument", arguments_as_written(call)[0]);
  check("null", info());
}
