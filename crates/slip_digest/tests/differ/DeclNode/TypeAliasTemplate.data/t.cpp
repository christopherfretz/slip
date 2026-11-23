#include "side.h"
// §5.3 DeclNode `TypeAliasTemplate.data`: a default template argument.
#ifdef B
template <class T = int> using A = T;
#else
template <class T> using A = T;
#endif
