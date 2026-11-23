namespace legacy {
int f(int);
int f(long);
struct S { int m() const; static int s(); };
int f(int x) { return x; }
}
int use() { return legacy::f(1) + legacy::S::s(); }
