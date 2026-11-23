#include "side.h"
// §7 "Parentheses", F2: a parenthesis around a non-dependent operand in a template's body is redundant and skipped.
int x;
template<class T> int f(T) {
#ifdef B
  return (x) + 1;
#else
  return x + 1;
#endif
}
