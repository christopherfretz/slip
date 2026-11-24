// WP-112 second fix round (digest §9 "Defaults at a use"): a default used in the body of a lambda that is the operand of an immediate invocation in another function's default argument; that invocation has no value at the declaration (Kiln evaluates it per use), so the walk writes the declaration's default by value, and the inner use's name with it: g's unit differs (clang prints "auto legacy::(anonymous class)::operator()() const")
#include "srcloc.h"
consteval auto pass(auto fn) { return fn; }
void log(std::source_location l = std::source_location::current());
using F = void (*)();
namespace acme {
void g(F f = pass(+[] { struct L { void m() { log(); } }; L{}.m(); log(); }));
void f() { g(); }
}
