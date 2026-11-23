// std: c++17
#include "side.h"
// §10.2 DeclNameKind `DeductionGuide`: added.
template <class T> struct S { S(T); };
#ifdef B
S(char) -> S<int>;
#endif
