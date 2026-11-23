// WP-112 R1, R2 (b) (digest §9): std::source_location::current() in a default argument (of a function, a function template, a class template's constructor; a default inside another default) is passed on at the use: the __impl object's function name is the caller's, fed to the names digest
#include "srcloc.h"
void take(const void *);
void log(std::source_location = std::source_location::current());
template <class T> void tlog(T, std::source_location = std::source_location::current());
template <class T> struct Logger { Logger(std::source_location l = std::source_location::current()) : fn(l.function_name()) {} const char *fn; };
struct Rec { std::source_location l; };
constexpr Rec inner(std::source_location l = std::source_location::current()) { return {l}; }
void outer(Rec = inner());
namespace acme {
void f() { log(); }
void g() { tlog(1); }
void h() { Logger<int> l; take(l.fn); }
void k() { outer(); }
}
