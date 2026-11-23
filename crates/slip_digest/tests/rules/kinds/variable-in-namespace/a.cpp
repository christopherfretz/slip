namespace acme {
int v = 1;
extern const int c;
const int c = 2;
struct S { static int s; };
int S::s = 3;
}
int use() { return acme::v + acme::c + acme::S::s; }
