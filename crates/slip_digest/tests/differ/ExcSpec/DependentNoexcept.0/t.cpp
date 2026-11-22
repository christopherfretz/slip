#include "side.h"
// §4.6 ExcSpec `DependentNoexcept.0`: the operand.
#ifdef B
template <class T> void f() noexcept(sizeof(T) > 2);
#else
template <class T> void f() noexcept(sizeof(T) > 1);
#endif
