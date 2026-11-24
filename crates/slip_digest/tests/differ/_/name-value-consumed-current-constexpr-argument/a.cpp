// WP-112 second fix round, G2 (digest §9 "Defaults at a use" (b)): std::source_location::current's argument is not a default or the builtin but pick(__builtin_source_location()), a constexpr function choosing by the name, so this is not (b): the builtin is consumed and written by value
#include "srcloc.h"
using I = std::source_location::__impl;
constexpr const I *pick(const I *p) { return p->_M_function_name[0] == 'v' && p->_M_function_name[5] == 'l' ? p : nullptr; }
void log(std::source_location l = std::source_location::current(pick(__builtin_source_location())));
namespace legacy {
void f() { log(); }
}
