namespace n {
void f(int);
class C { friend void f(int); int x = 0; };
}
void h() { n::f(1); }
