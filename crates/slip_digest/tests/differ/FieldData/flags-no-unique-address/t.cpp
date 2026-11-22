#include "side.h"
// std: c++26
// §5.5 FieldData `flags-no-unique-address`: set only by `define_aggregate` (`kiln_sema/src/meta/define.rs:614`).
#include "meta.h"
struct E {};
struct S;
#ifdef B
consteval {
  std::meta::define_aggregate(
      ^^S, {std::meta::data_member_spec(^^E, {.name = "e", .no_unique_address = false})});
}
#else
consteval {
  std::meta::define_aggregate(
      ^^S, {std::meta::data_member_spec(^^E, {.name = "e", .no_unique_address = true})});
}
#endif
