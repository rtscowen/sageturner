// CONSTRAINT: the file must be called sageturner.py, 
// so that the import statement works 
pub fn get_serve_code() -> String {
    
    let serve_code = r#"import sageturner
from fastapi import FastAPI, Request, Response, status
import uvicorn
model = sageturner.load()
app = FastAPI()
@app.get('/ping')
async def ping():
    if model: 
        return Response(status_code=status.HTTP_200_OK)
    else:
        raise HTTPException(status_code=500, detail="Error")
@app.post('/invocations')
async def predict(request: Request):
    body = await request.json()
    response = sageturner.predict(model, body)
    return response
if __name__ == "__main__":
    config = uvicorn.Config("serve:app", port=8080, host="0.0.0.0")
    server = uvicorn.Server(config=config)
    server.run()"#.to_string();

           println!("Serve code: ");
           print!("{serve_code}");
           serve_code
}
