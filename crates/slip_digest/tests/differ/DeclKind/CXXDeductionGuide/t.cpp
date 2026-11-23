// std: c++17
#include "side.h"
// §5.3 DeclKind `CXXDeductionGuide`: added.
template <class T> struct S { S(T); };
#ifdef B
S(char) -> S<int>;
#endif
