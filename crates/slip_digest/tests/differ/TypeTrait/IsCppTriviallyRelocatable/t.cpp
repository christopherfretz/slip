#include "side.h"
// §7 TypeTrait: `IsCppTriviallyRelocatable`: against `__builtin_is_structural`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __builtin_is_structural(T); };
#else
template <class T> struct A { unsigned long w = __builtin_is_cpp_trivially_relocatable(T); };
#endif
