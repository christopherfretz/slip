#include "side.h"
// §7 TypeTrait: `IsVolatile`: against `__can_pass_in_regs`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __can_pass_in_regs(T); };
#else
template <class T> struct A { unsigned long w = __is_volatile(T); };
#endif
