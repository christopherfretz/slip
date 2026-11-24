// WP-112 R1, R2 (a) (digest §9): __builtin_FUNCTION() in a default argument (of a function, a function template; a default inside another default) and in a default member initializer (an aggregate initialization) is passed on at the use: the caller's plain name, fed to the names digest
#include "srcloc.h"
void take(const char *);
void log(const char * = __builtin_FUNCTION());
template <class T> void tlog(T, const char * = __builtin_FUNCTION());
const char *inner(const char *p = __builtin_FUNCTION()) { return p; }
void outer(const char * = inner());
struct Agg { int x; const char *p = __builtin_FUNCTION(); };
void f2() { log(); }
void g2() { tlog(1); }
void k2() { outer(); }
void h2() { Agg a{1}; take(a.p); }
