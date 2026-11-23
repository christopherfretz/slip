namespace legacy {
struct T { int v; };
struct S {
  S();
  ~S();
  operator T() const;
  S &operator=(const S &);
  int operator()(int) const;
};
int operator""_u(unsigned long long);
bool operator==(T, T);
}
int use(legacy::S s) { return legacy::T(s).v + s(1); }
