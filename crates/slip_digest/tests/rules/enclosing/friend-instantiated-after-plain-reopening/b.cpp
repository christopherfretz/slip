namespace __attribute__((visibility("hidden"))) n {
template <class T> struct S {
  friend int f(S) { return 1; }
  template <class U> friend int g(S, U) { return 2; }
  friend int k(S);
};
}
namespace n {}
int u() { n::S<int> s; return f(s) + g(s, 1) + k(s); }
