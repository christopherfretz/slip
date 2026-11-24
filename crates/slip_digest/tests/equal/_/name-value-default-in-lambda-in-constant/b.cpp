// WP-112 second fix round, F1 (digest §9 "Defaults at a use"): a default used in the body of a lambda whose lambda-expression is the operand of an immediate invocation (pass, a consteval constructor), or in a member of a local class in such a body: the body is an evaluation context of its own, and the use passes the caller's name on, fed to the names digest (one immediate function per lambda, so that its instantiations' names are unique; h5: the same lambda with no immediate invocation; h6, h7: the builtin written in the body)
#include "srcloc.h"
void take(const char *);
consteval auto pass1(auto fn) { return fn; }
consteval auto pass2(auto fn) { return fn; }
consteval auto pass4(auto fn) { return fn; }
consteval auto pass6(auto fn) { return fn; }
consteval auto pass7(auto fn) { return fn; }
struct Cmd { void (*fn)(); consteval Cmd(void (*f)()) : fn(f) {} };
void log(std::source_location l = std::source_location::current());
void blog(const void * = __builtin_source_location());
namespace acme {
void h1() { auto l = pass1([] { log(); }); l(); }
void h2() { auto l = pass2([] { blog(); }); l(); }
void h4() { auto l = pass4([] { struct L { void m() { log(); } }; L{}.m(); }); l(); }
void m4() { Cmd c(+[] { log(); }); c.fn(); }
void h5() { auto l = [] { log(); }; l(); }
void h6() { auto l = pass6([] { take(std::source_location::current().function_name()); }); l(); }
void h7() { auto l = pass7([] { take(__PRETTY_FUNCTION__); }); l(); }
}
