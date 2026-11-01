const BASE_URL: string = import.meta.env.VITE_API_URL ?? "http://localhost:8000";

export async function search(collection: string, query: string): Promise<unknown> {
  const response = await fetch(`${BASE_URL}/${collection}/search?q=${encodeURIComponent(query)}`);
  return response.json();
}
