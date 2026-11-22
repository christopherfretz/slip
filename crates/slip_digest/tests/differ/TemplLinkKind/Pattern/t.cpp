#include "side.h"
// §9.1 TemplLinkKind `Pattern`: a class template's pattern
// is written in the template's unit with the tag of a pattern.
#ifdef B
template <class T> struct X {};
#else
struct X {};
#endif
