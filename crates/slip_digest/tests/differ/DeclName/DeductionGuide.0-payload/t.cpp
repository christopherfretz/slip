// std: c++17
#include "side.h"
// §10.2 DeclName `DeductionGuide.0-payload`: the template, through the discriminator.
#ifdef B
template <class T> struct U { U(T); }; U(char) -> U<int>;
#else
template <class T> struct S { S(T); }; S(char) -> S<int>;
#endif
