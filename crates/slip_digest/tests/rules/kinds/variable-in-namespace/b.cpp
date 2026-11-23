namespace legacy {
int v = 1;
extern const int c;
const int c = 2;
struct S { static int s; };
int S::s = 3;
}
int use() { return legacy::v + legacy::c + legacy::S::s; }
