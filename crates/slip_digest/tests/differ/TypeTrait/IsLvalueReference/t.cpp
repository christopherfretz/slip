#include "side.h"
// §7 TypeTrait: `IsLvalueReference`: against `__is_member_function_pointer`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_member_function_pointer(T); };
#else
template <class T> struct A { unsigned long w = __is_lvalue_reference(T); };
#endif
