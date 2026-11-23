#include "dep.h"

namespace acme {
struct Mine {};
template <class T> struct Box {};
} namespace legacy {
template <class T> struct traits<acme::Box<T>> {};
template <> constexpr int width<acme::Mine> = 4;
} namespace acme { template <> int id<Mine>(Mine) { return 1; } } namespace legacy {
namespace lib { template <> struct hash<acme::Mine> {}; }
}
