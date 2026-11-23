namespace a { namespace detail { int k = 3; } int use() { return detail::k; } }
namespace a { namespace detail { struct X { int k = 3; }; template <class T> struct Y { T t; }; } }
template <class T> struct W { T v; using F = int (*)(a::detail::X); operator int T::*() const { return nullptr; } operator F() const { return nullptr; } operator a::detail::Y<T>() const { return {v}; } operator W*() { return this; } };
struct S { int m; };
int f(W<S> w) { int S::*pm = w; W<S>::F fp = w; a::detail::Y<S> y = w; W<S> *self = w; return (pm == nullptr) + (fp == nullptr) + (self != nullptr) + y.t.m; }
