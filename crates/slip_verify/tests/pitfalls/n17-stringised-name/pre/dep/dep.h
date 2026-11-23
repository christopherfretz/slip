namespace legacy {
template <class T> int enroll(const char *name) { return name[0]; }
}
#define ENROLL(T) legacy::enroll<T>(#T)
