// WP-112 R1, R2 (a) (digest §9): __builtin_source_location() in a default argument (of a function, a function template, a class template's constructor; a default inside another default) is the operand of no immediate invocation, passed on at the use: the __impl object's function name is the caller's, fed to the names digest
#include "srcloc.h"
void take(const void *);
void log(const void * = __builtin_source_location());
template <class T> void tlog(T, const void * = __builtin_source_location());
template <class T> struct Logger { Logger(const void *p = __builtin_source_location()) : fn(p) {} const void *fn; };
constexpr const void *inner(const void *p = __builtin_source_location()) { return p; }
void outer(const void * = inner());
namespace acme {
void f() { log(); }
void g() { tlog(1); }
void h() { Logger<int> l; take(l.fn); }
void k() { outer(); }
}
