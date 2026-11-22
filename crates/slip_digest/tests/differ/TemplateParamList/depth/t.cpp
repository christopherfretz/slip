#include "side.h"
// §9.2 TemplateParamList `depth`: a member template's head is one level deeper
// than a template at namespace scope.
#ifdef B
template <class T> struct A { template <class U> struct M {}; };
#else
template <class T> struct A {};
template <class U> struct M {};
#endif
