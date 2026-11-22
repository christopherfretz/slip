#include "side.h"
// §7 TypeTrait: `IsImplicitLifetime`: against `__builtin_is_cpp_trivially_relocatable`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __builtin_is_cpp_trivially_relocatable(T); };
#else
template <class T> struct A { unsigned long w = __builtin_is_implicit_lifetime(T); };
#endif
