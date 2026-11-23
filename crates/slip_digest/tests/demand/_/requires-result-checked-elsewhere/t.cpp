#include "side.h"
// §7 _ `requires-result-checked-elsewhere`: the stored result `RequiresExpr.aux` is the expression's own: another requires-expression checking the same requirements on the same type, before and after (in functions: a variable added would move the order unit, digest §2.1).
#ifdef B
template<class T> bool rw = requires(T t) { t + 1; };
void j() { (void)rw<int>; }
template<class T> bool rv = requires(T t) { t + 1; };
bool i = rv<int>;
void k() { (void)rw<int>; }
#else
template<class T> bool rv = requires(T t) { t + 1; };
bool i = rv<int>;
#endif
