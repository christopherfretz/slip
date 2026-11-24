namespace a { struct S { int f() { return 1; } }; }
template <class T> int pick(T) { return 0; }
template <class T>
  requires requires(T t) { t.g(); }
int pick(T) { return 1; }
int u() { return pick(a::S{}) + a::S{}.f(); }
