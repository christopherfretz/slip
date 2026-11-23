#include "side.h"
// §5.3 DeclNode `ClassTemplate.data`: a default template argument.
#ifdef B
template <class T = int> struct S;
#else
template <class T> struct S;
#endif
