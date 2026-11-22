#include "side.h"
// §7 _ `requires-result-checked-elsewhere`: the stored result `RequiresExpr.aux` is the expression's own: another requires-expression checking the same requirements on the same type, before and after.
#ifdef B
template<class T> bool rw = requires(T t) { t + 1; };
bool j = rw<int>;
template<class T> bool rv = requires(T t) { t + 1; };
bool i = rv<int>;
bool k = rw<int>;
#else
template<class T> bool rv = requires(T t) { t + 1; };
bool i = rv<int>;
#endif
