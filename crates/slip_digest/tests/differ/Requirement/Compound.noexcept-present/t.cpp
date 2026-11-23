#include "side.h"
// §9.3 Requirement `Compound.noexcept-present`: a `noexcept` added to a compound requirement.
#ifdef B
template<class T> bool rv = requires(T t) { { t + 1 } noexcept; };

#else
template<class T> bool rv = requires(T t) { { t + 1 }; };

#endif
