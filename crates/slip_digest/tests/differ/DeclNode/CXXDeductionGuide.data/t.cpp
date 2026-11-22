// std: c++17
#include "side.h"
// §5.3 DeclNode `CXXDeductionGuide.data`: `explicit`.
template <class T> struct S { S(T); };
#ifdef B
explicit S(char) -> S<int>;
#else
S(char) -> S<int>;
#endif
