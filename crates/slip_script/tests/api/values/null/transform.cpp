// spec: §5.1
// The null `info` (spec/api.md §5.1): default-constructed, false, equal
// only to null; what a query that finds nothing gives (§4 rule 1); and the
// predicates accept it (§4 rule 7). A query given null where a value is
// required throws `invalid_handle` (§4).
#include <slip/meta.h>

using namespace slip;

consteval void say(bool b, std::string_view what) {
  report(severity::note, std::string(b ? "yes: " : "no: ") + std::string(what));
}

consteval void transform() {
  info null;
  info f = lookup("f")[0];
  say(!null, "a default info is null");
  say(static_cast<bool>(f), "a found entity is not null");
  say(null == info(), "two nulls are equal");
  say(null != f, "null is not an entity");
  say(lookup("nothing").empty(), "a name that names nothing gives an empty sequence");
  say(!parent_of(lookup("::")[0]), "the global namespace has a null parent");
  say(!is_entity(null) && !is_declaration(null) && !is_function(null) && !is_definition(null),
      "every predicate is false for null");
  try {
    (void)identifier_of(null);
    say(false, "identifier_of(null) throws");
  } catch (const error& e) {
    say(e.code() == errc::invalid_handle, "identifier_of(null) throws invalid_handle");
  }
}
