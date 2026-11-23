namespace legacy {
class C;
int g(int);
struct F { friend class C; friend int g(int); };
}
struct G { friend class legacy::C; };
