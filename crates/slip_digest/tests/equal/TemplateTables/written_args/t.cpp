#include "side.h"
// §9.5 TemplateTables `written_args`: how many arguments were written before
// defaults filled the rest.
template <class T, class U = int> struct X {};
#ifdef B
template struct X<char, int>;
#else
template struct X<char>;
#endif
