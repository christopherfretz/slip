#include "side.h"
// §4.6 ExcSpec `DependentNoexcept`: a dependent operand against `noexcept`.
#ifdef B
template <class T> void f() noexcept(sizeof(T) > 1);
#else
template <class T> void f() noexcept;
#endif
