// std: c++26
#include "side.h"
// §5.3 DeclNode `flags`: the record is written (`0-injected`: a member `define_aggregate` made, against none).
#include "meta.h"
struct S;
#ifdef B
consteval { std::meta::define_aggregate(^^S, {std::meta::data_member_spec(^^int, {.name = "e"})}); }
#endif
