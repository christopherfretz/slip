namespace acme {
struct S { int x; };
void f(S);
enum E { A, B };
int g(S s) { return s.x + A; }
}
