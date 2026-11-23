// std: c++20
#include "side.h"
// §4.4 "Spellings", rule 11, templated (`own_spelling::templated_at`): a lambda in a variable template's initialiser; its closure lives at namespace scope, so the `using enum`'s own chain never meets the template, and the stream (held by the variable template) is what says templated. Kiln keeps no written qualifier (`spelling/using-enum`).
template<int N> struct Q { using I = int; enum E { e1 }; };
template<int N> struct X { };
int *p;
X<8> *q;
int f(int);
#ifdef B
long f(long);
#endif
template<class T> int v = [] { using enum Q<sizeof(sizeof(f(T())))>::E; return 0; }();
