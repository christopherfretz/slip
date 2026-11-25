#include "side.h"
/* the stop at a substituted parameter replacement, made first by a sugared explicit argument or by int */
namespace lib { struct X {}; int make(X); }
lib::X x;
template <class T> struct Wr { T m; };
template <class T> int fe(Wr<T> *);
#ifdef B
int b = fe<int>(nullptr);
int a = fe<decltype(lib::make(x))>(nullptr);
#else
int a = fe<decltype(lib::make(x))>(nullptr);
int b = fe<int>(nullptr);
#endif
