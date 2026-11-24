namespace b { struct S {}; int f(S) { return 5; } }
namespace a {
int f(int x) { return x; }
template <class T> int call(T t) { return f(t); }
}
int u() { return a::call(1) + a::call(b::S{}); }
