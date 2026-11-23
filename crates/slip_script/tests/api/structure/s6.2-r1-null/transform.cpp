// spec: §6.2 rule 1
// Each predicate returns false for a null `info` and does not throw
// (spec/api.md §6.2 rule 1, §4 rule 7). The forged case is in
// `values/forged`.
#include <slip/meta.h>

using namespace slip;

#define CHECK(pred)                                                          \
  [&]() consteval -> std::string {                                           \
    try {                                                                    \
      return std::string(#pred) + (pred(info()) ? ": true" : ": false");     \
    } catch (const error&) {                                                 \
      return std::string(#pred) + ": threw";                                 \
    }                                                                        \
  }()

consteval void transform() {
  for (const std::string& s :
       {CHECK(is_entity), CHECK(is_declaration), CHECK(is_reference), CHECK(is_expression),
        CHECK(is_namespace), CHECK(is_type), CHECK(is_function), CHECK(is_variable),
        CHECK(is_template), CHECK(is_parameter), CHECK(is_definition), CHECK(is_call)})
    report(severity::note, s);
}
