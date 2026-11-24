// WP-112 second fix round (digest §9 "Defaults at a use"): an inherited constructor's default argument, used by the inheriting constructor at the construction in f: the caller's name is passed on
#include "srcloc.h"
struct Base { Base(int, std::source_location l = std::source_location::current()); };
struct Der : Base { using Base::Base; };
namespace legacy {
void f() { Der d(1); }
}
