#include "side.h"
// §9.2 TemplateInfo `params`: the head of a template.
#ifdef B
template <class T, class U> struct X {};
#else
template <class T> struct X {};
#endif
