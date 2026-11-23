// WP-045 with WP-104 (digest §9): names held in the resolved units of implicit instantiations (a member's <definition>, a static member's <value>)
#include "srcloc.h"
void take(const char *);
namespace legacy {
template <class T> constexpr const char *name() { return __PRETTY_FUNCTION__; }
template <class T> struct S {
  static constexpr const char *n = name<T>();
  const char *m() { take(__func__); take(std::source_location::current().function_name()); return __PRETTY_FUNCTION__; }
};
const char *f() { return S<int>().m(); }
const char *g() { return S<long>::n; }
}
