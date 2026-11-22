#include "side.h"
// §9.5 TemplateTables `preferred_named`: a template-id written after the
// `preferred_name` attribute marks the specialisation, a typedef written
// before it does not; either is how a name prints.
template <class T> struct Q;
using QI = Q<int>;
template <class T> struct [[clang::preferred_name(QI)]] Q {};
#ifdef B
QI* p;
#else
Q<int>* p;
#endif
