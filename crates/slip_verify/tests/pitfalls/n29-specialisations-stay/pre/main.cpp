#include "dep.h"

namespace legacy {
struct Mine {};
template <class T> struct Box {};

template <class T> struct traits<Box<T>> {};
template <> constexpr int width<Mine> = 4;
template <> int id<Mine>(Mine) { return 1; }
namespace lib { template <> struct hash<Mine> {}; }
}
