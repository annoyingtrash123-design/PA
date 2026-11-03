// Tiny key-value wrapper over IndexedDB (one database per name, one object store "kv").
// Every call degrades to a no-op/undefined if IndexedDB is unavailable (private windows etc.).

const open = (name: string): Promise<IDBDatabase> =>
  new Promise((resolve, reject) => {
    const req = indexedDB.open(name, 1);
    req.onupgradeneeded = () => req.result.createObjectStore("kv");
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });

const dbs = new Map<string, Promise<IDBDatabase>>();
const db = (name: string) => {
  let p = dbs.get(name);
  if (!p) dbs.set(name, (p = open(name)));
  return p;
};

export async function kvGet<T>(name: string, key: string): Promise<T | undefined> {
  try {
    const d = await db(name);
    return await new Promise<T | undefined>((resolve, reject) => {
      const r = d.transaction("kv", "readonly").objectStore("kv").get(key);
      r.onsuccess = () => resolve(r.result as T | undefined);
      r.onerror = () => reject(r.error);
    });
  } catch {
    return undefined;
  }
}

export async function kvSet(name: string, key: string, value: unknown): Promise<void> {
  try {
    const d = await db(name);
    await new Promise<void>((resolve, reject) => {
      const tx = d.transaction("kv", "readwrite");
      tx.objectStore("kv").put(value, key);
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
  } catch {
    // ignore: the app still works from memory
  }
}

export async function kvClear(name: string): Promise<void> {
  try {
    const d = await db(name);
    await new Promise<void>((resolve, reject) => {
      const tx = d.transaction("kv", "readwrite");
      tx.objectStore("kv").clear();
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
  } catch {
    // ignore
  }
}

export async function kvDelete(name: string, key: string): Promise<void> {
  try {
    const d = await db(name);
    await new Promise<void>((resolve, reject) => {
      const tx = d.transaction("kv", "readwrite");
      tx.objectStore("kv").delete(key);
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
  } catch {
    // ignore
  }
}
