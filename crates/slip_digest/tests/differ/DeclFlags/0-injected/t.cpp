// std: c++26
#include "side.h"
// §5.3 DeclFlags `0-injected`: a member `define_aggregate` made against one written.
#include "meta.h"
#ifdef B
struct S;
consteval { std::meta::define_aggregate(^^S, {std::meta::data_member_spec(^^int, {.name = "e"})}); }
#else
struct S { int e; };
#endif
