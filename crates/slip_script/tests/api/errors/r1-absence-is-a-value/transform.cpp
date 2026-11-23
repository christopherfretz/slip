// spec: §4 rule 1
// Absence is a value (spec/api.md §4 rule 1): a query that finds nothing
// returns a null `info` (`parent_of` the global namespace), a null `range`
// (`name_range_of` a name from a macro's replacement list), or an empty
// sequence (`lookup` of an absent name, `references_to` an entity no name
// refers to, `declarations_of` a specialisation, `parameters_of` a
// function with none). None throws.
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


consteval void transform() {
  report(severity::note, "parent_of(::): " + OUTCOME(parent_of(lookup("::")[0])) +
                             (parent_of(lookup("::")[0]) ? ", not null" : ", null"));
  info x = lookup("x")[0];
  range r = name_range_of(references_to(x)[0]);
  report(severity::note, std::string("name_range_of(a name from a macro body): ") + (r ? "a range" : "null"));
  report(severity::note, "lookup(\"absent\"): " + num(lookup("absent").size()));
  report(severity::note, "references_to(unreferenced): " + num(references_to(lookup("u")[0]).size()));
  info spec = callee_of(calls_to(lookup("tf")[0])[0]);
  report(severity::note, "declarations_of(specialisation): " + num(declarations_of(spec).size()));
  report(severity::note, "parameters_of(none): " + num(parameters_of(lookup("none")[0]).size()));
}
