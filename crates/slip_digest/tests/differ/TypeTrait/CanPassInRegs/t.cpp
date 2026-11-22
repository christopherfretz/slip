#include "side.h"
// §7 TypeTrait: `CanPassInRegs`: against `__is_bitwise_cloneable`, the result value-dependent on both sides.
#ifdef B
template <class T> struct A { unsigned long w = __is_bitwise_cloneable(T); };
#else
template <class T> struct A { unsigned long w = __can_pass_in_regs(T); };
#endif
