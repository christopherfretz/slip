// WP-112 R2 (c) (digest §9): mine::source_location::current is not std::source_location::current (the callee is identified by the entity, not by its name): an immediate invocation that consumes the builtin, so the caller's pretty name is written by value and differs under the rename
#include "srcloc.h"
void take(const char *);
namespace mine {
struct source_location {
  static consteval source_location current(const std::source_location::__impl *p = __builtin_source_location()) noexcept {
    source_location s; s.impl = p; return s; }
  constexpr const char *function_name() const noexcept { return impl ? impl->_M_function_name : ""; }
  const std::source_location::__impl *impl = nullptr;
};
}
void log(mine::source_location = mine::source_location::current());
void start() { log(); }
