#pragma once
namespace d {
inline int g(...) { return 0; }
template <class T> int call(T t) { return g(t); }
}
