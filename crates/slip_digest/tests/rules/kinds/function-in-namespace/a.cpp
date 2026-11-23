namespace acme {
int f(int);
int f(long);
struct S { int m() const; static int s(); };
int f(int x) { return x; }
}
int use() { return acme::f(1) + acme::S::s(); }
