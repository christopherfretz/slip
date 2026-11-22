#include "side.h"
// §9.5 TemplateTables `args_as_written`: the arguments of an explicit
// specialisation as written (an alias in them is spelling).
using Int = int;
template <class T> struct X {};
#ifdef B
template <> struct X<Int> {};
#else
template <> struct X<int> {};
#endif
