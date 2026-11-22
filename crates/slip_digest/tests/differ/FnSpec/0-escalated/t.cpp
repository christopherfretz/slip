// std: c++20
#include "side.h"
// §5.4 FnSpec `0-escalated`: a lambda's call operator that calls a consteval function becomes immediate. Outside a closure type only an instantiation escalates, and its bit is in `definition` (§11.9; demand/_/escalated-on-demand); an explicit instantiation of an escalated function is an error.
consteval int id(int x) { return x; }
#ifdef B
auto v = [](int x) { return id(x); };
#else
auto v = [](int x) { return x; };
#endif
